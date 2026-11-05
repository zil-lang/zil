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
            TypeKind::Named { path, args } => {
                if path.len() == 1 {
                    for instance in &self.products.storage.instances {
                        self.budget.node(ty.span, depth)?;
                        if let zil_ir::Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                            && let zil_ir::Type::Record { name, .. } | zil_ir::Type::Enum { name, .. } = storage.as_ref()
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
        if !self.qualified_actual_syntax(ty, depth)? {
            return Ok(None);
        }
        if let TypeKind::Group(inner) = &mut ty.kind {
            return self.qualified_type_actual(inner, depth + 1);
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
                && let zil_ir::Type::Record { name: physical, .. } | zil_ir::Type::Enum { name: physical, .. } = storage.as_ref()
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
