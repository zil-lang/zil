use super::*;

pub(super) fn scalar(name: &str) -> Option<CtScalarTy> {
    Some(match name {
        "void" => CtScalarTy::Unit,
        "bool" => CtScalarTy::Bool,
        "u8" => CtScalarTy::U8,
        "u16" => CtScalarTy::U16,
        "i32" => CtScalarTy::I32,
        "i64" => CtScalarTy::I64,
        _ => return None,
    })
}
pub(super) fn category(name: &str) -> Option<CtSyntaxCategory> {
    Some(match name {
        "Expr" => CtSyntaxCategory::Expr,
        "Type" => CtSyntaxCategory::Type,
        "StmtList" => CtSyntaxCategory::StmtList,
        "ItemList" => CtSyntaxCategory::ItemList,
        _ => return None,
    })
}
fn ct_only(ty: &s::TypeRef) -> bool {
    match &ty.kind {
        s::TypeKind::Named { path, args } => {
            path.last().is_some_and(|n| {
                matches!(
                    n.as_str(),
                    "Type"
                        | "Syntax"
                        | "Ident"
                        | "TokenTree"
                        | "TokenCursor"
                        | "Text"
                        | "ShapeInfo"
                        | "FieldInfo"
                        | "ImplInfo"
                        | "LayoutInfo"
                )
            }) || args
                .iter()
                .any(|a| matches!(a, s::GenericArg::Type(t) if ct_only(t)))
        }
        s::TypeKind::Tuple(ts) | s::TypeKind::Union(ts) | s::TypeKind::Impl(ts) => {
            ts.iter().any(ct_only)
        }
        s::TypeKind::Group(t)
        | s::TypeKind::Optional(t)
        | s::TypeKind::Reference { inner: t, .. }
        | s::TypeKind::Array { element: t, .. }
        | s::TypeKind::ForLifetime { inner: t, .. } => ct_only(t),
        s::TypeKind::Function { params, result }
        | s::TypeKind::ForeignFunction { params, result, .. } => {
            params.iter().any(ct_only) || ct_only(result)
        }
        s::TypeKind::Error { error, result } => ct_only(error) || ct_only(result),
        _ => false,
    }
}
pub(super) fn ct_only_function(function: &s::Function) -> bool {
    function.params.iter().any(|p| ct_only(&p.ty))
        || function.return_type.as_ref().is_some_and(ct_only)
}
impl<'a> Compiler<'a> {
    pub(super) fn named_item(
        &mut self,
        current: usize,
        path: &[String],
        span: Span,
    ) -> R<(usize, &'a s::Item)> {
        let (module, name) = self.resolve_path(current, path, span)?;
        let mut found = None;
        for item in &self.modules[module].syntax.items {
            self.budget.work(1, span)?;
            let actual = match &item.kind {
                s::ItemKind::Aggregate(a) => Some(&a.name),
                s::ItemKind::Enum(e) => Some(&e.name),
                s::ItemKind::TypeAlias(a) => Some(&a.name),
                _ => None,
            };
            if actual == Some(&name) {
                if found.is_some() {
                    return Err(error(span, "duplicate CT type declaration"));
                }
                if current != module && !item.public {
                    return Err(error(span, "CT type is private to its defining module"));
                }
                found = Some(item);
            }
        }
        found
            .map(|item| (module, item))
            .ok_or_else(|| error(span, "unknown CT type name"))
    }
    pub(super) fn ty(&mut self, module: usize, ty: &s::TypeRef, depth: usize) -> R<CtTy> {
        self.budget.depth(depth, ty.span)?;
        Ok(match &ty.kind {
            s::TypeKind::Group(inner) => self.ty(module, inner, depth + 1)?,
            s::TypeKind::Tuple(fields) if fields.is_empty() => CtTy::Scalar(CtScalarTy::Unit),
            s::TypeKind::Tuple(fields) => {
                let mut out = Vec::new();
                for t in fields {
                    self.budget.node(t.span)?;
                    out.push(self.ty(module, t, depth + 1)?);
                }
                CtTy::Tuple(out)
            }
            s::TypeKind::Array { element, length } => {
                let element = Box::new(self.ty(module, element, depth + 1)?);
                match length {
                    None => CtTy::List(element),
                    Some(expr) => CtTy::Array {
                        element,
                        length: self.fixed_length(expr)?,
                    },
                }
            }
            s::TypeKind::Named { path, args } => {
                if path.len() == 1 {
                    if let Some(scalar) = scalar(&path[0]) {
                        if !args.is_empty() {
                            return Err(error(ty.span, "scalar CT type takes no arguments"));
                        }
                        return Ok(CtTy::Scalar(scalar));
                    }
                    let simple = match path[0].as_str() {
                        "Type" => Some(CtTy::Type),
                        "Ident" => Some(CtTy::Ident),
                        "TokenTree" => Some(CtTy::TokenTree),
                        "TokenCursor" => Some(CtTy::TokenCursor),
                        "Text" => Some(CtTy::Text),
                        "ShapeInfo" => Some(CtTy::ShapeInfo),
                        "FieldInfo" => Some(CtTy::FieldInfo),
                        "ImplInfo" => Some(CtTy::ImplInfo),
                        "LayoutInfo" => Some(CtTy::LayoutInfo),
                        _ => None,
                    };
                    if let Some(simple) = simple {
                        if !args.is_empty() {
                            return Err(error(ty.span, "atomic CT type takes no arguments"));
                        }
                        return Ok(simple);
                    }
                    if path[0] == "Syntax" {
                        if let [
                            s::GenericArg::Type(s::TypeRef {
                                kind: s::TypeKind::Named { path, args },
                                ..
                            }),
                        ] = args.as_slice()
                            && path.len() == 1
                            && args.is_empty()
                            && let Some(category) = category(&path[0])
                        {
                            return Ok(CtTy::Syntax(category));
                        }
                        return Err(error(
                            ty.span,
                            "Syntax needs exactly one fixed syntax category",
                        ));
                    }
                    if path[0] == "List" {
                        if let [s::GenericArg::Type(inner)] = args.as_slice() {
                            return Ok(CtTy::List(Box::new(self.ty(module, inner, depth + 1)?)));
                        }
                        return Err(error(ty.span, "CT List requires one element type"));
                    }
                }
                if !args.is_empty() {
                    return Err(error(
                        ty.span,
                        "CT data requires an actual concrete nominal instance",
                    ));
                }
                let (owner, item) = self.named_item(module, path, ty.span)?;
                match &item.kind {
                    s::ItemKind::TypeAlias(alias) => self.ty(
                        owner,
                        alias.value.as_ref().ok_or_else(|| {
                            error(ty.span, "CT alias has no actual type definition")
                        })?,
                        depth + 1,
                    )?,
                    s::ItemKind::Aggregate(a) => {
                        if a.kind != s::AggregateKind::Struct || !a.generics.is_empty() {
                            return Err(error(
                                ty.span,
                                "CT data cannot contain runtime resources, packages or unresolved type parameters",
                            ));
                        }
                        for (id, (m, actual)) in self.record_sources.iter().enumerate() {
                            self.budget.work(1, ty.span)?;
                            if *m == owner && std::ptr::eq(*actual, a) {
                                return Ok(CtTy::Record(CtRecordSchemaId(id as u32)));
                            }
                        }
                        self.budget.node(ty.span)?;
                        let id = CtRecordSchemaId(self.records.len() as u32);
                        let name = self.symbol(owner, item.span)?;
                        self.record_sources.push((owner, a));
                        self.records.push(CtRecordSchema {
                            name,
                            fields: Vec::new(),
                            origin: CtSchemaOrigin::UserPure,
                        });
                        let mut fields = Vec::new();
                        for field in &a.fields {
                            self.budget.node(field.span)?;
                            if field.manages {
                                return Err(error(
                                    field.span,
                                    "CT data cannot hold a managed runtime resource",
                                ));
                            }
                            fields.push(self.ty(owner, &field.ty, depth + 1)?);
                        }
                        self.records[id.0 as usize].fields = fields;
                        CtTy::Record(id)
                    }
                    s::ItemKind::Enum(e) => {
                        if !e.generics.is_empty() {
                            return Err(error(ty.span, "CT enum requires a concrete instance"));
                        }
                        for (id, (m, actual)) in self.enum_sources.iter().enumerate() {
                            self.budget.work(1, ty.span)?;
                            if *m == owner && std::ptr::eq(*actual, e) {
                                return Ok(CtTy::Enum(CtEnumSchemaId(id as u32)));
                            }
                        }
                        self.budget.node(ty.span)?;
                        let id = CtEnumSchemaId(self.enums.len() as u32);
                        let name = self.symbol(owner, item.span)?;
                        self.enum_sources.push((owner, e));
                        self.enums.push(CtEnumSchema {
                            name,
                            variants: Vec::new(),
                            origin: CtSchemaOrigin::UserPure,
                        });
                        let mut variants = Vec::new();
                        for variant in &e.variants {
                            self.budget.node(variant.span)?;
                            let mut fields = Vec::new();
                            match &variant.fields {
                                s::VariantFields::Unit => {}
                                s::VariantFields::Tuple(fs) => {
                                    for field in fs {
                                        self.budget.node(field.span)?;
                                        if field.manages {
                                            return Err(error(
                                                field.span,
                                                "CT enum field cannot manage runtime resources",
                                            ));
                                        }
                                        fields.push(self.ty(owner, &field.ty, depth + 1)?);
                                    }
                                }
                                s::VariantFields::Record(fs) => {
                                    for field in fs {
                                        self.budget.node(field.span)?;
                                        if field.manages {
                                            return Err(error(
                                                field.span,
                                                "CT enum field cannot manage runtime resources",
                                            ));
                                        }
                                        fields.push(self.ty(owner, &field.ty, depth + 1)?);
                                    }
                                }
                            }
                            variants.push(fields);
                        }
                        self.enums[id.0 as usize].variants = variants;
                        CtTy::Enum(id)
                    }
                    _ => unreachable!(),
                }
            }
            _ => {
                return Err(error(
                    ty.span,
                    "CT-only data has no runtime address, resource, closure or foreign ABI",
                ));
            }
        })
    }
    fn fixed_length(&mut self, expression: &s::Expr) -> R<u32> {
        self.budget.work(1, expression.span)?;
        if let s::ExprKind::Literal(s::Literal::Integer(text)) = &expression.kind {
            self.budget.text(text, expression.span)?;
            let value = zil_types::parse_integer_literal(
                text,
                Some(&zil_ir::Type::I32),
                false,
                expression.span,
            )?;
            if let Constant::I32(n) = value
                && n >= 0
            {
                return Ok(n as u32);
            }
        }
        Err(error(
            expression.span,
            "CT array type requires a nonnegative fixed integer length",
        ))
    }
}
